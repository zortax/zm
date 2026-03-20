#!/bin/bash

# ==========================================
# Configuration
# ==========================================
CONTAINER_NAME="local_mail_test"
LOGIN="test"
EMAIL="test@localhost"
PASS="password"
IMAP_PORT=3143
SMTP_PORT=3025

echo "Starting local IMAP/SMTP server using Greenmail via Docker..."

# Stop and remove the existing container if it's already running
if docker ps -a --format '{{.Names}}' | grep -Eq "^${CONTAINER_NAME}\$"; then
    echo "Cleaning up old container ${CONTAINER_NAME}..."
    docker rm -f $CONTAINER_NAME >/dev/null
fi

# Run the Greenmail container with host networking so its 127.0.0.1 bindings
# are directly reachable from the host (Greenmail doesn't support binding to 0.0.0.0).
docker run -d --name $CONTAINER_NAME \
  --network host \
  -e GREENMAIL_OPTS="-Dgreenmail.setup.test.all=true -Dgreenmail.users=${LOGIN}:${PASS}@localhost" \
  greenmail/standalone:2.0.0 >/dev/null

echo "Container started. Waiting for services to initialize..."

# ==========================================
# Inline Python Script to Populate Data
# ==========================================
python3 - <<'PYEOF'
import imaplib
import smtplib
import time
import email.utils
from datetime import datetime, timedelta, timezone

SMTP_PORT = 3025
IMAP_PORT = 3143
EMAIL = "test@localhost"
LOGIN = "test"
PASS = "password"

def make_date(days_ago=0, hour=9, minute=0):
    """Create an RFC 2822 date string for N days ago at the given time."""
    dt = datetime.now(timezone.utc).replace(hour=hour, minute=minute, second=0, microsecond=0)
    dt -= timedelta(days=days_ago)
    return email.utils.format_datetime(dt)

# 1. Wait for the SMTP port to accept connections
ready = False
for _ in range(30):
    try:
        with smtplib.SMTP('127.0.0.1', SMTP_PORT) as s:
            s.noop()
        ready = True
        break
    except Exception:
        time.sleep(1)

if not ready:
    print("Error: Mail server did not become ready in time.")
    exit(1)

print("Services are ready. Populating demo emails and folders...")

# 2. Send one initial email via SMTP to register the user in Greenmail
with smtplib.SMTP('127.0.0.1', SMTP_PORT) as s:
    from email.message import EmailMessage
    msg = EmailMessage()
    msg.set_content("Welcome to your new mailbox!")
    msg['Subject'] = "Welcome"
    msg['From'] = "system@localhost"
    msg['To'] = EMAIL
    msg['Date'] = make_date(days_ago=30, hour=8)
    s.send_message(msg)

time.sleep(1)

# 3. Connect via IMAP and build everything
try:
    mail = imaplib.IMAP4('127.0.0.1', IMAP_PORT)
    mail.login(LOGIN, PASS)

    # -- Create standard folders --
    for folder in ['Sent', 'Drafts', 'Trash', 'Archive', 'Junk']:
        mail.create(folder)

    # -- Create custom folders (one level of nesting only) --
    mail.create('Work')
    mail.create('Work.Projects')
    mail.create('Work.Invoices')
    mail.create('Shopping')
    mail.create('Travel')
    mail.create('Travel.Bookings')

    def append_msg(folder, sender_name, sender_email, subject, body, days_ago=0, hour=9, minute=0, flags=None):
        date_str = make_date(days_ago, hour, minute)
        raw = (
            f"From: {sender_name} <{sender_email}>\r\n"
            f"To: {EMAIL}\r\n"
            f"Subject: {subject}\r\n"
            f"Date: {date_str}\r\n"
            f"MIME-Version: 1.0\r\n"
            f"Content-Type: text/plain; charset=utf-8\r\n"
            f"\r\n"
            f"{body}"
        )
        imap_date = imaplib.Time2Internaldate(
            time.mktime((datetime.now() - timedelta(days=days_ago)).timetuple())
        )
        flag_str = flags if flags else None
        mail.append(folder, flag_str, imap_date, raw.encode('utf-8'))

    # =========================================
    # INBOX - diverse emails across time ranges
    # =========================================

    # -- Today --
    append_msg('INBOX',
        'Alice Chen', 'alice.chen@acme.corp',
        'Quick question about the API endpoint',
        'Hey,\n\nI was looking at the /users endpoint and noticed it returns a 500 when\nthe page parameter is negative. Is that expected behavior or should we\nadd validation?\n\nThanks,\nAlice',
        days_ago=0, hour=10, minute=42)

    append_msg('INBOX',
        'GitHub', 'notifications@github.com',
        '[rust-lang/rust] Fix lifetime elision in async fn (PR #12847)',
        'rust-bot commented on this pull request:\n\n> LGTM, but can we add a test for the edge case where the lifetime is\n> bound to a higher-ranked trait bound?\n\nView it on GitHub:\nhttps://github.com/rust-lang/rust/pull/12847',
        days_ago=0, hour=9, minute=15)

    append_msg('INBOX',
        'Bob Martinez', 'bob@designstudio.io',
        'Updated mockups for the dashboard',
        'Hi there,\n\nI\'ve updated the mockups based on our call yesterday. The main changes are:\n\n- Simplified the navigation sidebar\n- Added a dark mode variant\n- Moved the notifications bell to the top-right\n\nLet me know what you think. Happy to jump on a call if needed.\n\nCheers,\nBob',
        days_ago=0, hour=8, minute=3)

    # -- Yesterday --
    append_msg('INBOX',
        'Linear', 'notifications@linear.app',
        'ZM-142: Implement folder tree in sidebar',
        'Sarah assigned you to ZM-142\n\nImplement folder tree in sidebar\nPriority: High\nStatus: In Progress\n\n---\nView issue: https://linear.app/zm/issue/ZM-142',
        days_ago=1, hour=16, minute=30, flags='\\Seen')

    append_msg('INBOX',
        'Clara Wong', 'clara.wong@bigcorp.com',
        'Re: Partnership proposal',
        'Thanks for sending this over. I\'ve shared it with our BD team and\nthey\'re very interested. Can we schedule a 30-min call next week to\ndiscuss the technical integration?\n\nBest,\nClara',
        days_ago=1, hour=14, minute=12, flags='\\Flagged')

    append_msg('INBOX',
        'Stripe', 'receipts@stripe.com',
        'Your receipt from Acme Corp',
        'Receipt for your payment of $49.00\n\nDescription: Pro Plan - Monthly\nDate: March 20, 2026\nPayment method: Visa ending in 4242\n\nIf you have questions, contact support@acme.corp',
        days_ago=1, hour=11, minute=0, flags='\\Seen')

    append_msg('INBOX',
        'David Park', 'david.park@university.edu',
        'Lecture notes from yesterday',
        'Hi everyone,\n\nAttached are the lecture notes from yesterday\'s session on distributed\nsystems. Key topics covered:\n\n1. CAP theorem and its practical implications\n2. Consensus algorithms (Raft vs Paxos)\n3. Eventual consistency patterns\n\nThe recording will be available on the course portal by Friday.\n\nProf. Park',
        days_ago=1, hour=9, minute=45, flags='\\Seen')

    # -- This week --
    append_msg('INBOX',
        'Emma Fischer', 'emma@freelance.dev',
        'Invoice #2026-038 attached',
        'Hi,\n\nPlease find attached my invoice for the frontend work completed this\nmonth. Summary:\n\n- Component library migration: 24h\n- Accessibility audit fixes: 8h\n- Performance optimization: 12h\n\nTotal: 44h @ $120/h = $5,280.00\n\nPayment terms: Net 30\n\nThanks,\nEmma',
        days_ago=3, hour=10, minute=0, flags='\\Seen')

    append_msg('INBOX',
        'Hacker News', 'hn@ycombinator.com',
        'HN Daily Digest - Top Stories',
        'Top stories for today:\n\n1. Show HN: A terminal-based email client written in Rust (342 points)\n2. SQLite as a document database (289 points)\n3. Why we switched from Kubernetes to bare metal (201 points)\n4. The unreasonable effectiveness of plain text (178 points)\n\nRead more at https://news.ycombinator.com',
        days_ago=4, hour=18, minute=0, flags='\\Seen')

    # -- Older --
    append_msg('INBOX',
        'Frank Liu', 'frank.liu@startup.io',
        'Team offsite planning',
        'Hey team,\n\nI\'m starting to plan our Q2 offsite. Current thinking:\n\n- Location: somewhere in Portugal\n- Dates: June 15-19\n- Focus: architecture review + team building\n\nPlease fill out the preference survey by end of this week.\n\nFrank',
        days_ago=8, hour=15, minute=30, flags='\\Seen')

    append_msg('INBOX',
        'AWS', 'no-reply@aws.amazon.com',
        'Your March billing summary',
        'Your AWS account billing summary for March 2026:\n\nTotal charges: $127.43\n\nTop services:\n  EC2: $78.20\n  RDS: $32.10\n  S3: $8.93\n  CloudFront: $5.20\n  Other: $3.00\n\nView detailed billing: https://console.aws.amazon.com/billing',
        days_ago=12, hour=6, minute=0, flags='\\Seen')

    append_msg('INBOX',
        'Grace Kim', 'grace.kim@example.com',
        'Book recommendation: Designing Data-Intensive Applications',
        'Hey!\n\nJust finished reading DDIA by Martin Kleppmann. Absolutely phenomenal\nbook if you\'re into distributed systems. The chapters on replication\nand partitioning changed how I think about database design.\n\nHighly recommend it!\n\nGrace',
        days_ago=21, hour=20, minute=15, flags='(\\Seen \\Flagged)')

    # =========================================
    # Sent
    # =========================================
    append_msg('Sent',
        LOGIN, EMAIL,
        'Re: Quick question about the API endpoint',
        'Hey Alice,\n\nGood catch! That\'s definitely a bug. I\'ll add input validation for\nthe pagination parameters. Should have a fix up by EOD.\n\nThanks for flagging it.',
        days_ago=0, hour=11, minute=5, flags='\\Seen')

    append_msg('Sent',
        LOGIN, EMAIL,
        'Re: Partnership proposal',
        'Hi Clara,\n\nGreat to hear! How about next Tuesday at 2pm EST? I can send a\nZoom link.\n\nLooking forward to it.',
        days_ago=1, hour=15, minute=0, flags='\\Seen')

    append_msg('Sent',
        LOGIN, EMAIL,
        'Deployment checklist for v2.4',
        'Team,\n\nHere\'s the deployment checklist for v2.4 going out on Friday:\n\n1. Run database migrations\n2. Deploy auth service first\n3. Roll out API servers (canary 10% -> 50% -> 100%)\n4. Verify health checks\n5. Run smoke tests\n\nPlease review and add anything I missed.',
        days_ago=3, hour=14, minute=30, flags='\\Seen')

    # =========================================
    # Drafts
    # =========================================
    append_msg('Drafts',
        LOGIN, EMAIL,
        'Blog post: Building a mail client in Rust',
        'Draft outline:\n\n## Introduction\n- Why build a mail client from scratch?\n- The Rust ecosystem for email (async-imap, mail-parser, lettre)\n\n## Architecture\n- GPUI for the UI layer\n- SQLite for local cache\n- Sync engine design\n\n## Challenges\n- IMAP protocol quirks\n- Handling different server implementations\n\n[TODO: flesh out each section]',
        days_ago=2, hour=22, minute=0)

    # =========================================
    # Work
    # =========================================
    append_msg('Work',
        'Helen Torres', 'helen@acme.corp',
        'Quarterly OKR review',
        'Hi team,\n\nReminder that our quarterly OKR review is this Friday at 3pm.\nPlease have your self-assessments ready.\n\nKey results to discuss:\n- API latency p99 < 200ms (currently at 180ms)\n- Test coverage > 80% (currently at 76%)\n- Zero critical incidents (we had 1)\n\nHelen',
        days_ago=5, hour=9, minute=0, flags='\\Seen')

    # =========================================
    # Work.Projects
    # =========================================
    append_msg('Work.Projects',
        'Ian Nakamura', 'ian@acme.corp',
        'Project Falcon: status update',
        'Weekly update for Project Falcon:\n\nCompleted:\n- Database schema migration\n- Auth service refactor\n- Load testing (passed at 10k rps)\n\nIn progress:\n- Frontend dashboard (70% done)\n- API documentation\n\nBlocked:\n- Waiting on security review for the new OAuth flow\n\nETA for launch: April 5th',
        days_ago=2, hour=16, minute=0, flags='\\Seen')

    append_msg('Work.Projects',
        'Julia Andersen', 'julia@acme.corp',
        'Re: Project Falcon: status update',
        'Thanks Ian. The security review should be done by Wednesday.\nI\'ll ping the team to prioritize it.\n\nRegarding the frontend - can we get a demo before the all-hands?\n\nJulia',
        days_ago=1, hour=10, minute=30, flags='\\Seen')

    # =========================================
    # Work.Invoices
    # =========================================
    append_msg('Work.Invoices',
        'Accounting', 'accounting@acme.corp',
        'Invoice #INV-2026-0312 processed',
        'Invoice #INV-2026-0312 has been processed.\n\nVendor: CloudFlare, Inc.\nAmount: $2,340.00\nPeriod: March 2026\nStatus: Paid\n\nPlease retain this for your records.',
        days_ago=7, hour=11, minute=0, flags='\\Seen')

    # =========================================
    # Shopping
    # =========================================
    append_msg('Shopping',
        'Amazon', 'order-update@amazon.com',
        'Your order has shipped!',
        'Your order #112-4839271-8834621 has shipped!\n\nItems:\n- Keychron K3 Pro Mechanical Keyboard (1x) - $89.99\n- USB-C to USB-C Cable 2-pack (1x) - $12.99\n\nEstimated delivery: March 23-25\nTracking: 1Z999AA10123456784',
        days_ago=1, hour=13, minute=22, flags='\\Seen')

    append_msg('Shopping',
        'Bandcamp', 'noreply@bandcamp.com',
        'New release from Tycho',
        'Tycho just released a new album: "Infinite Coast"\n\nYou\'re receiving this because you follow Tycho on Bandcamp.\n\nListen now: https://tycho.bandcamp.com/album/infinite-coast\n\n"A shimmering, sun-drenched collection of downtempo electronica."',
        days_ago=6, hour=17, minute=0, flags='\\Seen')

    # =========================================
    # Travel
    # =========================================
    append_msg('Travel',
        'Kayak', 'alerts@kayak.com',
        'Price alert: SFO to LIS dropped to $487',
        'Good news! A flight you\'re tracking has dropped in price.\n\nSan Francisco (SFO) -> Lisbon (LIS)\nJune 14 - June 22\nPrice: $487 (was $612)\nAirline: TAP Air Portugal\n\nBook now before prices change!',
        days_ago=3, hour=7, minute=30, flags='\\Flagged')

    # =========================================
    # Travel.Bookings
    # =========================================
    append_msg('Travel.Bookings',
        'Airbnb', 'automated@airbnb.com',
        'Booking confirmed: Lisbon apartment',
        'Your reservation is confirmed!\n\nProperty: Sunny Alfama Studio with River View\nHost: Maria S.\nCheck-in: June 14, 2026 (3:00 PM)\nCheck-out: June 22, 2026 (11:00 AM)\nTotal: $892.00\n\nAddress will be shared 48 hours before check-in.\n\nHave a great trip!',
        days_ago=2, hour=19, minute=45, flags='\\Seen')

    # =========================================
    # Archive
    # =========================================
    append_msg('Archive',
        'Kevin O\'Brien', 'kevin@oldproject.org',
        'Wrapping up the migration project',
        'Hi all,\n\nThe migration project is officially complete. Final stats:\n\n- 2.4M records migrated\n- Zero data loss\n- 99.97% uptime during migration\n- Completed 3 days ahead of schedule\n\nThanks everyone for the hard work. I\'ve archived the project docs\nin Confluence.\n\nKevin',
        days_ago=45, hour=16, minute=0, flags='\\Seen')

    append_msg('Archive',
        'LinkedIn', 'notifications@linkedin.com',
        'Congratulations on your work anniversary!',
        'You\'re celebrating 3 years at Acme Corp!\n\nYour connections are sending you congratulations.\n\nSee who\'s celebrating with you: https://linkedin.com/notifications',
        days_ago=60, hour=9, minute=0, flags='\\Seen')

    # Delete the initial welcome email from INBOX (it was only needed to register the user)
    mail.select('INBOX')
    result, data = mail.search(None, 'SUBJECT', '"Welcome"')
    if result == 'OK' and data[0]:
        for num in data[0].split():
            mail.store(num, '+FLAGS', '\\Deleted')
        mail.expunge()

    mail.logout()
    print("Successfully populated mailbox with demo data!")

except Exception as e:
    print(f"IMAP Population Error: {e}")

PYEOF

# ==========================================
# Client Configuration Output
# ==========================================
echo ""
echo "================================================="
echo "Local Mail Server is UP and Populated!"
echo "================================================="
echo "Configure zm with these settings:"
echo ""
echo "  Email Address: $EMAIL"
echo "  Username:      $LOGIN"
echo "  Password:      $PASS"
echo ""
echo "Incoming Server (IMAP):"
echo "  Host: 127.0.0.1"
echo "  Port: $IMAP_PORT"
echo "  Security: None / Unencrypted"
echo ""
echo "Outgoing Server (SMTP):"
echo "  Host: 127.0.0.1"
echo "  Port: $SMTP_PORT"
echo "  Security: None / Unencrypted"
echo "================================================="
echo "To stop and discard the server later, run:"
echo "  docker rm -f $CONTAINER_NAME"
